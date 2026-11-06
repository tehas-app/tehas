//! The selection over scratch trees with fake test files of their own, so what
//! is selected does not depend on the real suite. test/select_test.sh runs it
//! on the real workspace and through scripts/verify.sh --changed.

use super::{select, RUNS};
use std::fs;
use tehas_core::testdir::TestDir;

struct Tree(TestDir);

impl Tree {
    fn new() -> Tree {
        let t = TestDir::new("xtask-select");
        fs::create_dir_all(t.join("test/fixtures/headless-x")).unwrap();
        fs::write(t.join("test/lib.sh"), "").unwrap();
        Tree(t)
    }

    /// A file of the tree, its directories made.
    fn file(&self, path: &str, text: &str) -> &Tree {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
        self
    }

    /// A fake test file that names <text>.
    fn fake(&self, name: &str, text: &str) -> &Tree {
        self.file(
            &format!("test/{name}"),
            &format!("#!/usr/bin/env bash\n# {text}\n"),
        )
    }

    fn pick_with(&self, paths: &[&str], metadata: Option<&str>) -> String {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        select(self.0.path(), &paths, || metadata.map(str::to_string)).join("\n")
    }

    fn pick(&self, paths: &[&str]) -> String {
        self.pick_with(paths, None)
    }

    fn pick_ws(&self, paths: &[&str]) -> String {
        self.pick_with(paths, Some(WORKSPACE))
    }
}

/// A workspace of the real shape, as cargo metadata --no-deps gives it. report
/// depends on the store for its tests, summary on report to build.
const WORKSPACE: &str = r#"{"workspace_root": "/w", "packages": [
  {"name": "console", "manifest_path": "/w/Cargo.toml", "dependencies": [
    {"name": "serde_json", "kind": null},
    {"name": "tehas-core", "kind": null, "path": "/w/core"}]},
  {"name": "store", "manifest_path": "/w/store/Cargo.toml", "dependencies": [
    {"name": "tehas-core", "kind": null, "path": "/w/core"}]},
  {"name": "tehas-core", "manifest_path": "/w/core/Cargo.toml", "dependencies": []},
  {"name": "tehas-dispatch", "manifest_path": "/w/dispatch/Cargo.toml", "dependencies": [
    {"name": "tehas-core", "kind": null, "path": "/w/core"}]},
  {"name": "tehas-mcp", "manifest_path": "/w/mcp/Cargo.toml", "dependencies": [
    {"name": "tehas-core", "kind": null, "path": "/w/core"}]},
  {"name": "report", "manifest_path": "/w/report/Cargo.toml", "dependencies": [
    {"name": "store", "kind": "dev", "path": "/w/store"}]},
  {"name": "summary", "manifest_path": "/w/summary/Cargo.toml", "dependencies": [
    {"name": "report", "kind": "build", "path": "/w/report"}]}
]}"#;

#[test]
fn a_changed_test_file_selects_itself_and_nothing_else() {
    let t = Tree::new();
    t.fake("a_test.sh", "lib/config.sh")
        .fake("b_test.sh", "nothing");
    assert_eq!(t.pick(&["test/b_test.sh"]), "test test/b_test.sh");
    assert_eq!(t.pick(&["./test/b_test.sh"]), "test test/b_test.sh");
}

#[test]
fn a_file_name_with_a_dot_counts_as_a_word_and_only_as_a_whole_word() {
    let t = Tree::new();
    t.fake("a_test.sh", ". \"$T_ROOT/lib/config.sh\"")
        .fake("b_test.sh", "lib/myconfig.sh")
        .fake("c_test.sh", "config.shx and xconfig.sh");
    assert_eq!(t.pick(&["lib/config.sh"]), "test test/a_test.sh");
}

#[test]
fn a_command_counts_after_its_directory_after_tehas_or_as_a_test_files_name() {
    let t = Tree::new();
    t.fake("a_test.sh", "\"$T_ROOT/libexec/update\" --check")
        .fake("b_test.sh", "t_run \"tehas update\"")
        .fake("update_release_test.sh", "nothing")
        .fake("c_test.sh", "an update of the list")
        .fake("d_test.sh", "tehas updater")
        .fake("e_test.sh", "tehasbot update\n")
        .fake("f_test.sh", "tehas update-x");
    assert_eq!(
        t.pick(&["libexec/update"]),
        "test test/a_test.sh\ntest test/b_test.sh\ntest test/e_test.sh\ntest test/update_release_test.sh"
    );
}

#[test]
fn a_test_that_loops_over_every_command_is_selected_for_any_command() {
    let t = Tree::new();
    t.fake(
        "a_test.sh",
        "for f in \"$T_ROOT\"/libexec/*; do bash -n \"$f\"; done; libexec/\"$name\"",
    );
    assert_eq!(t.pick(&["libexec/work"]), "test test/a_test.sh");
}

#[test]
fn a_helper_that_names_the_path_brings_in_the_test_files_that_use_it() {
    let t = Tree::new();
    t.file("test/disk_lib.sh", "# runs lib/disk.sh\n")
        .fake("a_test.sh", ". disk_lib.sh")
        .fake("b_test.sh", "nothing");
    assert_eq!(t.pick(&["lib/disk.sh"]), "test test/a_test.sh");
}

#[test]
fn a_changed_helper_selects_the_test_files_that_name_it() {
    let t = Tree::new();
    t.file("test/disk_lib.sh", "# a helper\n")
        .fake("a_test.sh", ". disk_lib.sh")
        .fake("b_test.sh", "nothing");
    assert_eq!(t.pick(&["test/disk_lib.sh"]), "test test/a_test.sh");
}

#[test]
fn a_path_under_console_selects_the_workspace_and_the_shell_tests_that_build_it() {
    let t = Tree::new();
    t.fake("a_test.sh", "t_store || t_skip")
        .fake("b_test.sh", "console/target")
        .fake("c_test.sh", "nothing")
        .fake("d_test.sh", "t_attach");
    assert_eq!(
        t.pick(&["console/store/src/main.rs"]),
        "cargo\ntest test/a_test.sh\ntest test/b_test.sh\ntest test/d_test.sh"
    );
}

#[test]
fn a_change_in_one_crate_with_no_dependents_selects_that_crate() {
    let t = Tree::new();
    assert_eq!(
        t.pick_ws(&["console/mcp/src/tools.rs", "console/mcp/tests/rpc.rs"]),
        "crate tehas-mcp"
    );
}

#[test]
fn a_change_in_the_store_selects_what_depends_on_it_at_any_depth_of_any_kind() {
    let t = Tree::new();
    assert_eq!(
        t.pick_ws(&["console/store/src/db.rs"]),
        "crate console\ncrate report\ncrate store\ncrate summary\ncrate tehas-dispatch"
    );
}

#[test]
fn a_crate_whose_tests_run_the_stores_program_selects_the_store_and_no_more() {
    let t = Tree::new();
    assert_eq!(
        t.pick_ws(&["console/dispatch/src/poll.rs"]),
        "crate store\ncrate tehas-dispatch"
    );
    assert_eq!(
        t.pick_ws(&["console/src/health.rs"]),
        "crate console\ncrate store"
    );
    assert_eq!(
        t.pick_ws(&["console/tests/console.rs"]),
        "crate console\ncrate store"
    );
}

#[test]
fn the_shell_tests_of_a_change_under_console_come_after_its_crates() {
    let t = Tree::new();
    t.fake("a_test.sh", "t_store || t_skip");
    assert_eq!(
        t.pick_ws(&["console/mcp/src/tools.rs"]),
        "crate tehas-mcp\ntest test/a_test.sh"
    );
}

#[test]
fn core_the_manifests_and_the_lock_select_the_whole_workspace() {
    let t = Tree::new();
    for p in [
        "console/core/src/lib.rs",
        "console/Cargo.toml",
        "console/Cargo.lock",
        "console/mcp/Cargo.toml",
    ] {
        assert_eq!(t.pick_ws(&["console/mcp/src/tools.rs", p]), "cargo", "{p}");
        assert_eq!(t.pick_ws(&[p, "console/mcp/src/tools.rs"]), "cargo", "{p}");
    }
}

#[test]
fn a_path_in_no_crate_selects_the_whole_workspace() {
    let t = Tree::new();
    for p in [
        "console/rust-toolchain.toml",
        "console/.cargo/config.toml",
        "console/newcrate/src/lib.rs",
        "console/mcp",
    ] {
        assert_eq!(t.pick_ws(&[p]), "cargo", "{p}");
    }
}

#[test]
fn when_cargo_cannot_say_what_the_crates_are_the_whole_workspace() {
    let t = Tree::new();
    for metadata in [
        None,
        Some("not json"),
        Some("{}"),
        Some(r#"{"workspace_root": "/w", "packages": []}"#),
    ] {
        assert_eq!(
            t.pick_with(&["console/mcp/src/tools.rs"], metadata),
            "cargo",
            "{metadata:?}"
        );
    }
}

#[test]
fn metadata_is_asked_once() {
    let t = Tree::new();
    let mut asked = 0;
    let paths = vec![
        "console/mcp/src/a.rs".to_string(),
        "console/store/src/b.rs".to_string(),
    ];
    select(t.0.path(), &paths, || {
        asked += 1;
        None
    });
    assert_eq!(asked, 1);
}

#[test]
fn a_crate_of_the_list_of_programs_that_is_gone_selects_the_whole_workspace() {
    let t = Tree::new();
    let mut v: serde_json::Value = serde_json::from_str(WORKSPACE).unwrap();
    v["packages"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["name"] != "tehas-dispatch");
    let less = v.to_string();
    assert_eq!(
        t.pick_with(&["console/mcp/src/tools.rs"], Some(&less)),
        "cargo"
    );
}

#[test]
fn the_list_of_programs_names_crates() {
    for (user, program) in RUNS {
        assert!(!user.is_empty() && !program.is_empty());
    }
}

#[test]
fn a_fixture_selects_the_test_files_that_name_its_directory() {
    let t = Tree::new();
    t.fake(
        "a_test.sh",
        "\"$T_ROOT/test/fixtures/headless-x/progress.jsonl\"",
    )
    .fake("b_test.sh", "nothing");
    assert_eq!(
        t.pick(&["test/fixtures/headless-x/progress.jsonl"]),
        "test test/a_test.sh"
    );
}

#[test]
fn a_changelog_fragment_selects_the_test_files_that_name_changelog_d() {
    let t = Tree::new();
    t.fake("a_test.sh", "nothing")
        .fake("b_test.sh", "reads changelog.d/ beside the file");
    assert_eq!(
        t.pick(&["changelog.d/1606.md", "changelog.d/my-branch.md"]),
        "test test/b_test.sh"
    );
}

#[test]
fn a_front_page_file_selects_the_node_test_that_loads_it() {
    let t = Tree::new();
    t.file(
        "test/brick_wall_test.mjs",
        "// import(\"../host/fun/brick-wall.js\")\n",
    )
    .fake("a_test.sh", "nothing");
    assert_eq!(
        t.pick(&["host/fun/brick-wall.js"]),
        "node test/brick_wall_test.mjs"
    );
}

#[test]
fn the_heavy_files_a_shared_library_reaches_are_left_to_the_full_gate() {
    let t = Tree::new();
    for f in [
        "install_steps_test.sh",
        "update_test.sh",
        "update_release_test.sh",
        "pack_release_test.sh",
        "bootstrap_test.sh",
        "install_test.sh",
        "a_test.sh",
    ] {
        t.fake(f, ". \"$T_ROOT/lib/config.sh\"");
    }
    assert_eq!(
        t.pick(&["lib/config.sh"]),
        "test test/a_test.sh
omit test/bootstrap_test.sh
omit test/install_steps_test.sh
test test/install_test.sh
omit test/pack_release_test.sh
omit test/update_release_test.sh
omit test/update_test.sh"
    );
}

#[test]
fn a_heavy_file_runs_when_the_change_touches_what_it_tests() {
    let t = Tree::new();
    t.fake(
        "install_steps_test.sh",
        "\"$T_ROOT/install.sh\"; \"$T_ROOT/libexec/update\"",
    )
    .fake("update_test.sh", "\"$T_ROOT/libexec/update\"")
    .fake(
        "pack_release_test.sh",
        "\"$T_ROOT/scripts/pack-release.sh\"",
    )
    .fake("bootstrap_test.sh", "\"$T_ROOT/bootstrap.sh\"");
    assert_eq!(
        t.pick(&["libexec/update", "scripts/pack-release.sh", "bootstrap.sh"]),
        "test test/bootstrap_test.sh
omit test/install_steps_test.sh
test test/pack_release_test.sh
test test/update_test.sh"
    );
    assert_eq!(t.pick(&["install.sh"]), "test test/install_steps_test.sh");
}

#[test]
fn a_heavy_file_runs_when_it_or_a_helper_it_uses_changed() {
    let t = Tree::new();
    t.file("test/install_lib.sh", "# a helper\n")
        .fake("install_steps_test.sh", ". install_lib.sh")
        .fake("update_test.sh", "nothing");
    assert_eq!(
        t.pick(&["test/install_lib.sh", "test/update_test.sh"]),
        "test test/install_steps_test.sh\ntest test/update_test.sh"
    );
}

#[test]
fn the_gates_own_files_ask_for_the_full_gate() {
    let t = Tree::new();
    t.fake("a_test.sh", "nothing");
    for p in [
        "scripts/test.sh",
        "scripts/verify.sh",
        "scripts/select-tests.sh",
        "test/lib.sh",
        "console/xtask/src/select.rs",
        "console/xtask/Cargo.toml",
    ] {
        assert_eq!(
            t.pick_ws(&[p, "test/a_test.sh"]),
            format!("full {p} is part of the gate itself")
        );
        assert_eq!(
            t.pick_ws(&["test/a_test.sh", p]),
            format!("full {p} is part of the gate itself")
        );
    }
}

#[test]
fn a_path_no_test_names_asks_for_the_full_gate() {
    let t = Tree::new();
    t.fake("a_test.sh", "nothing");
    assert_eq!(
        t.pick(&["host/sandbox/Dockerfile"]),
        "full no test names host/sandbox/Dockerfile"
    );
}

#[test]
fn records_under_docs_and_the_top_level_notes_need_no_check() {
    let t = Tree::new();
    t.fake("a_test.sh", "nothing");
    assert_eq!(t.pick(&["docs/plans/x.md", "README.md"]), "");
    assert_eq!(
        t.pick(&["host/docs/x.md"]),
        "full no test names host/docs/x.md"
    );
}

#[test]
fn a_deleted_test_file_selects_nothing() {
    let t = Tree::new();
    t.fake("a_test.sh", "nothing");
    assert_eq!(t.pick(&["test/gone_test.sh"]), "");
}

#[test]
fn a_directory_with_a_slash_counts_for_the_files_in_it() {
    let t = Tree::new();
    t.fake("a_test.sh", "grep -r x \"$T_ROOT/host/docs\"")
        .fake("b_test.sh", "nothing");
    assert_eq!(t.pick(&["host/docs/tools.md"]), "test test/a_test.sh");
}

#[test]
fn hidden_files_under_test_are_not_test_files() {
    let t = Tree::new();
    t.fake(".x_test.sh", "lib/config.sh")
        .fake("a_test.sh", "lib/config.sh");
    assert_eq!(t.pick(&["lib/config.sh"]), "test test/a_test.sh");
}
