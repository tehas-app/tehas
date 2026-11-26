//! The manifest fixtures of api/fixtures/, judged by the reader. The files
//! are vendored, never fetched: the registry set carries its provenance and
//! this test reads only what is on disk.
#![cfg(test)]
use super::manifest::{self, Manifest};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn dir() -> PathBuf {
    format!("{}/../../api/fixtures/manifest", env!("CARGO_MANIFEST_DIR")).into()
}

/// The name a manifest carries, which the reader's pin check wants as the
/// name it was pinned under: the registry pins `specs` from the repository
/// `tehas-plugin-mattpocock-skills`, so each entry is judged by its own name.
fn name_of(bytes: &[u8]) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| "floorplan".into())
}

fn cases(folder: &str) -> Value {
    let text = fs::read_to_string(dir().join(folder).join("cases.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn names(folder: &str, suffix: &str) -> Vec<PathBuf> {
    let mut v: Vec<_> = fs::read_dir(dir().join(folder))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_str().unwrap().ends_with(suffix))
        .filter(|p| p.file_name().unwrap() != "cases.json")
        .collect();
    v.sort();
    v
}

/// Every fixture of `valid/` reads, and the two that prove the widest shape
/// keep what they carry: the counter-assertions of `invalid/` live in the
/// same folder, one rule away.
#[test]
fn the_valid_fixtures_read() {
    let files = names("valid", ".json");
    assert!(files.len() >= 4, "{files:?}");
    let mut read_ok = Vec::new();
    for p in &files {
        let bytes = fs::read(p).unwrap();
        let m: Manifest = manifest::read(&bytes, &name_of(&bytes))
            .unwrap_or_else(|w| panic!("{} refused: {w:?}", p.display()));
        read_ok.push(p.file_name().unwrap().to_str().unwrap().to_string());
        if p.file_name().unwrap() == "full.json" {
            assert_eq!(
                (
                    m.views.len(),
                    m.hooks.len(),
                    m.actions.len(),
                    m.skills.len()
                ),
                (2, 1, 1, 1)
            );
            assert_eq!(m.wasm[0].source.as_deref(), Some("src/deep/dir/sub"));
            assert_eq!(m.view_wasm[0].source.as_deref(), Some("src/view/"));
            assert_eq!(m.solid.as_deref(), Some("1.8.1"));
            assert_eq!(m.origins[0].methods, ["GET"]);
        }
        if p.file_name().unwrap() == "adapter.json" {
            assert_eq!(m.kind, super::manifest::Kind::Adapter);
            assert!(m.adapter.is_some());
        }
    }
    // The counter-assertion: not one of them is silently missing from the
    // folder, and none of the invalid ones read.
    assert_eq!(
        read_ok,
        [
            "adapter.json",
            "full.json",
            "minimum.json",
            "source-trailing.json"
        ]
    );
}

/// Each fixture of `invalid/` is refused with exactly the reasons its
/// `cases.json` records: the table of refusals is the contract, and a
/// refusal that ever stopped happening fails here.
#[test]
fn the_invalid_fixtures_are_refused_as_their_cases_say() {
    let cases = cases("invalid");
    let files = names("invalid", ".json");
    assert_eq!(files.len(), cases.as_object().unwrap().len());
    for p in &files {
        let name = p.file_name().unwrap().to_str().unwrap();
        let want: Vec<String> = cases[name]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: no case"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let bytes = fs::read(p).unwrap();
        let got = manifest::read(&bytes, &name_of(&bytes))
            .err()
            .unwrap_or_else(|| panic!("{name}: it read"));
        assert_eq!(got, want, "{name}");
    }
    // The both-way boundary of the source pattern: valid/ holds the two
    // that pass, invalid/ the two that do not, under the same rule.
    assert!(dir().join("valid").join("source-trailing.json").exists());
    for f in [
        "source-bare.json",
        "source-outside.json",
        "path-trailing.json",
    ] {
        assert!(dir().join("invalid").join(f).exists());
    }
}

/// The registry set, vendored with its provenance: each entry is judged by
/// the reader alone, and `cases.json` holds the verdict and the whole list
/// of reasons. Nothing here opens a socket: tehas reads what it fetched.
#[test]
fn the_vendored_registry_set_is_read_without_a_fetch() {
    let cases = cases("registry");
    let files = names("registry", ".tehas-plugin.json");
    assert_eq!(files.len(), cases.as_object().unwrap().len(), "19 entries");
    assert_eq!(files.len(), 19, "19 entries");
    for p in &files {
        let name = p.file_name().unwrap().to_str().unwrap();
        let case = &cases[name];
        // Every entry carries the record of where its bytes came from.
        let prov: Value = serde_json::from_str(
            &fs::read_to_string(
                p.with_file_name(name.replace(".tehas-plugin.json", ".provenance.json")),
            )
            .unwrap(),
        )
        .unwrap();
        for field in ["repo", "file", "sha", "fetched"] {
            assert!(
                prov.get(field).is_some(),
                "{name}: provenance has no {field}"
            );
        }
        let bytes = fs::read(p).unwrap();
        let got = manifest::read(&bytes, &name_of(&bytes));
        match case["verdict"].as_str().unwrap() {
            "accept" => {
                let m = got.unwrap_or_else(|w| panic!("{name}: refused: {w:?}"));
                let want: Vec<String> = case["asks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                assert_eq!(m.asks, want, "{name}: asks");
            }
            _ => {
                let refusals = got.err().unwrap_or_else(|| panic!("{name}: it read"));
                let want: Vec<&str> = case["reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::as_str)
                    .map(Option::unwrap)
                    .collect();
                assert_eq!(refusals, want, "{name}");
            }
        }
    }
    // The counter-assertion both ways: six of nineteen read, and the twelve
    // that do not are all refused for their major.
    let accepts = cases
        .as_object()
        .unwrap()
        .values()
        .filter(|c| c["verdict"] == "accept")
        .count();
    assert_eq!(accepts, 6);
    for c in cases
        .as_object()
        .unwrap()
        .values()
        .filter(|c| c["verdict"] == "refuse")
    {
        assert!(
            c["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r.as_str().unwrap() == "plugin is 1: this tehas reads manifest version 2"),
            "{c:?}"
        );
    }
}
