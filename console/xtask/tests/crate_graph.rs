//! The workspace's crate graph (R8) against its one written owner, the
//! "Crate graph" table of docs/architecture.md. The table is held to
//! `cargo metadata`, so it cannot go stale, and the graph is held to the
//! direction R8 asks for: an edge points inward only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The layers of the graph, inner to outer (the T6 cut, section 7, for the
/// crates there are today). A normal edge points to an earlier layer.
// TODO(T6 step 3): tehas-sandbox, above tehas-core; step 4: tehas-decide,
// above tehas-core; step 5: tehas-host, below tehas-sandbox; step 7:
// tehas-testkit, a dev edge of every crate.
const LAYERS: &[&[&str]] = &[
    &[
        "tehas-ports",
        "tehas-wiki-render",
        "tehas-describe",
        "tehas-review-render",
    ],
    &["tehas-core", "tehas-wiki-collect"],
    &["attach"],
    &[
        "tehas-github",
        "store",
        "analytics",
        "tehas-items",
        "tehas-host-feed",
        "tehas-mesh-feed",
        "tehas-front",
    ],
    &["tehas-mcp", "tehas-dispatch", "tehas-cycle"],
    &["tehas-ops"],
    &["tehas-plugin", "tehas-xtask"],
    &["tehas-plugin-gate", "console"],
];

/// The pure crates, and the only outside crates they may use.
// TODO(T6 step 3): tehas-core joins when tehas-sandbox takes plugin/out.rs,
// core's one libc use; step 4: tehas-decide joins.
const PURE: &[&str] = &["tehas-ports"];
const PURE_MAY_USE: &[&str] = &["serde", "serde_json", "regex", "sha2"];

/// The adapter a library may not depend on: only a program's wiring does.
const ADAPTER: &str = "tehas-github";

const HEADER: &str = "| Crate | Depends on | Its tests also use |\n| --- | --- | --- |";
const NOTHING: &str = "nothing of the workspace";

#[derive(Default, PartialEq, Debug)]
struct Edges {
    normal: BTreeSet<String>,
    dev: BTreeSet<String>,
}

struct Crate {
    edges: Edges,
    outside: BTreeSet<String>,
    has_bin: bool,
}

fn console() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The workspace's crates from `cargo metadata`, with their edges to each
/// other and the outside crates of their normal edges.
fn workspace() -> BTreeMap<String, Crate> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(console().join("Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata is JSON");
    let packages = meta["packages"].as_array().expect("metadata has packages");
    let names: BTreeSet<String> = packages
        .iter()
        .map(|p| {
            p["name"]
                .as_str()
                .expect("a package has a name")
                .to_string()
        })
        .collect();
    let mut crates = BTreeMap::new();
    for p in packages {
        let mut krate = Crate {
            edges: Edges::default(),
            outside: BTreeSet::new(),
            has_bin: p["targets"].as_array().into_iter().flatten().any(|t| {
                t["kind"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|k| k == "bin")
            }),
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let name = d["name"].as_str().expect("a dependency has a name");
            // A build edge counts as normal: it runs the crate's code too.
            let dev = d["kind"].as_str() == Some("dev");
            match (names.contains(name), dev) {
                (true, true) => krate.edges.dev.insert(name.into()),
                (true, false) => krate.edges.normal.insert(name.into()),
                (false, false) => krate.outside.insert(name.into()),
                (false, true) => false,
            };
        }
        crates.insert(p["name"].as_str().unwrap().to_string(), krate);
    }
    crates
}

fn cell(names: &BTreeSet<String>, empty: &str) -> String {
    if names.is_empty() {
        return empty.into();
    }
    names.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// The table as it should be: one row per crate, inner layers first.
fn table(crates: &BTreeMap<String, Crate>) -> String {
    let mut rows = vec![HEADER.to_string()];
    let mut order: Vec<&String> = crates.keys().collect();
    order.sort_by_key(|n| (layer(n), n.as_str()));
    for name in order {
        let e = &crates[name].edges;
        let dev = cell(&e.dev, "");
        let dev = if dev.is_empty() {
            " |".into()
        } else {
            format!(" {dev} |")
        };
        rows.push(format!("| {name} | {} |{dev}", cell(&e.normal, NOTHING)));
    }
    rows.join("\n")
}

fn names(cell: &str) -> BTreeSet<String> {
    let cell = cell.trim();
    if cell.is_empty() || cell == NOTHING {
        return BTreeSet::new();
    }
    cell.split(',').map(|n| n.trim().to_string()).collect()
}

/// The rows of docs/architecture.md's "Crate graph" table.
fn written() -> BTreeMap<String, Edges> {
    let page = fs::read_to_string(console().join("../docs/architecture.md")).unwrap();
    let section = page
        .split("\n## Crate graph\n")
        .nth(1)
        .expect("docs/architecture.md has no section \"Crate graph\"");
    let section = section.split("\n## ").next().unwrap_or("");
    let mut rows = BTreeMap::new();
    let lines = section.lines().filter(|l| l.starts_with('|'));
    for line in lines.skip(2) {
        let cells: Vec<&str> = line.split('|').collect();
        assert_eq!(cells.len(), 5, "a row of three cells: {line}");
        let edges = Edges {
            normal: names(cells[2]),
            dev: names(cells[3]),
        };
        let crate_name = cells[1].trim().to_string();
        assert!(
            rows.insert(crate_name.clone(), edges).is_none(),
            "the table has {crate_name} twice"
        );
    }
    rows
}

fn layer(name: &str) -> usize {
    LAYERS
        .iter()
        .position(|l| l.contains(&name))
        .unwrap_or_else(|| panic!("{name} is in no layer of LAYERS"))
}

#[test]
fn the_architecture_page_holds_the_crate_graph() {
    let crates = workspace();
    let actual: BTreeMap<&String, &Edges> = crates.iter().map(|(n, c)| (n, &c.edges)).collect();
    let page = written();
    let page: BTreeMap<&String, &Edges> = page.iter().collect();
    assert!(
        actual == page,
        "docs/architecture.md's \"Crate graph\" table differs from cargo metadata. \
         The table as it should be:\n\n{}\n",
        table(&crates)
    );
}

#[test]
fn every_crate_is_in_one_layer_and_every_layer_names_a_crate() {
    let crates = workspace();
    let listed: Vec<&str> = LAYERS.iter().flat_map(|l| l.iter().copied()).collect();
    let unique: BTreeSet<&str> = listed.iter().copied().collect();
    assert_eq!(listed.len(), unique.len(), "a crate is in two layers");
    for name in crates.keys() {
        layer(name);
    }
    for name in unique {
        assert!(
            crates.contains_key(name),
            "LAYERS names {name}, not a crate"
        );
    }
}

#[test]
fn a_normal_edge_points_inward() {
    for (name, krate) in workspace() {
        for to in &krate.edges.normal {
            assert!(
                layer(to) < layer(&name),
                "{name} -> {to} points outward or across its layer (R8)"
            );
        }
    }
}

#[test]
fn a_pure_crate_uses_only_the_allowed_outside_crates() {
    let crates = workspace();
    for name in PURE {
        for dep in &crates[*name].outside {
            assert!(
                PURE_MAY_USE.contains(&dep.as_str()),
                "{name} is pure and uses {dep}, not one of {PURE_MAY_USE:?} (R3, R8)"
            );
        }
    }
}

#[test]
fn only_a_program_depends_on_the_adapter() {
    for (name, krate) in workspace() {
        if krate.edges.normal.contains(ADAPTER) {
            assert!(
                krate.has_bin,
                "{name} is a library with no program and depends on {ADAPTER}: \
                 only a program's wiring may (R2, R8)"
            );
        }
    }
}
