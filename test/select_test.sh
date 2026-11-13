#!/usr/bin/env bash
# Tests for scripts/select-tests.sh, the paths-to-checks map behind
# `scripts/verify.sh --changed`, on the real workspace, and for that mode of the
# gate. How each path maps is tested beside the code, in
# console/xtask/src/select/tests.rs. The gate's cases run over a scratch
# checkout with fake test files of its own, so what is selected does not depend
# on the real suite.
# shellcheck disable=SC2016  # fake files and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

repo=$T/checkout
T_ENV=(PATH="$T/bin:/usr/bin:/bin" TEHAS_TEST_JOBS=1 TMPDIR="$T")

# The task, built once. The scratch checkout has no workspace, so its
# select-tests.sh runs this build over the checkout.
t_xtask || {
  echo "FAIL: $t_xtask_why"
  exit 1
}

fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/test/fixtures/headless-x" "$repo/lib" "$repo/libexec" "$repo/docs" "$repo/host/fun"
  cp "$T_ROOT/scripts/test.sh" "$T_ROOT/scripts/verify.sh" "$repo/scripts/"
  printf '#!/usr/bin/env bash\nexec "%s" select-tests --root "$(cd "$(dirname "$0")/.." && pwd)" "$@"\n' "$T_XTASK" >"$repo/scripts/select-tests.sh"
  chmod +x "$repo/scripts/select-tests.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-generic.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-bash-lines.sh"
  chmod +x "$repo/scripts/check-generic.sh" "$repo/scripts/check-bash-lines.sh"
  : >"$repo/test/lib.sh"
  t_stub shellcheck 'exit 0'
  rm -f "$T/bin/cargo" "$T/cargo-args" "$T/cargo-fails"
}

# A fake test file that names <text> and leaves a mark when it runs.
fake() { # name text
  printf '#!/usr/bin/env bash\n# %s\ntouch "$T/ran-%s"\necho "%s: 1 cases, 0 failed"\n' "$2" "$1" "$1" >"$repo/test/$1"
}

# A cargo over a workspace of the real shape, as cargo metadata --no-deps gives
# it. report depends on the store for its tests, summary on report to build.
# `cargo test` writes down its arguments.
workspace() {
  cat >"$T/metadata.json" <<'EOF'
{"workspace_root": "/w", "packages": [
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
]}
EOF
  t_stub cargo 'case "$1" in
  metadata) cat "$T/metadata.json" ;;
  test)
    echo "$*" >>"$T/cargo-args"
    [ ! -e "$T/cargo-fails" ] || exit 101
    ;;
esac'
}

# The real workspace, read by the real cargo.
real_pick() {
  bash "$T_ROOT/scripts/select-tests.sh" "$@" 2>&1 | grep -E '^(cargo|crate )'
}
real_crates() {
  cargo metadata --offline --no-deps --format-version 1 --manifest-path "$T_ROOT/console/Cargo.toml"
}

t_case "the real workspace: an MCP change selects the MCP server and the crate whose tests read with it"
# analytics/tests/sessions_contract.rs reads the snapshot with the MCP server's
# tools, so analytics dev-depends on tehas-mcp.
t_eq "$(real_pick console/mcp/src/tools.rs)" "crate analytics
crate tehas-mcp"

t_case "the real workspace: a store change selects the crates that run the store"
t_eq "$(real_pick console/store/src/db.rs)" "crate console
crate store
crate tehas-dispatch
crate tehas-ops"

t_case "the real workspace: a core change selects the whole workspace"
t_eq "$(real_pick console/core/src/lib.rs)" "cargo"

t_case "the real workspace: each crate of cargo metadata is selected by a path in it"
while IFS=$'\t' read -r name dir; do
  case "$name" in
    console) path=console/src/x.rs ;;
    tehas-core | tehas-xtask) continue ;;
    *) path=$dir/src/x.rs ;;
  esac
  t_contains "$(real_pick "$path")" "crate $name"
done < <(real_crates | jq -r '.workspace_root as $r | .packages[] |
  [.name, "console" + (.manifest_path | ltrimstr($r) | rtrimstr("/Cargo.toml"))] | @tsv')

# A crate's tests that run a sibling's program find it from current_exe. A crate
# that starts to has to be added to `runs` in the selector. tehas-cycle's one call
# finds the lib/ beside its libexec/, no program, so it stays off the list while
# that is what the call does. The xtask's one call names itself, to run each
# check of scripts/test.sh as its own test-task, no sibling's program.
t_case "the real workspace: the list of programs names every crate that calls current_exe"
dirs=$(real_crates | jq -r '.workspace_root as $r | .packages[] |
  [(.manifest_path | ltrimstr($r) | rtrimstr("/Cargo.toml") | ltrimstr("/")), .name] | @tsv')
own_lib=$T_ROOT/console/cycle/src/main.rs
t_contains "$(grep -A2 current_exe "$own_lib")" '.join("lib")'
own_task=$T_ROOT/console/xtask/src/main.rs
t_contains "$(grep -A5 current_exe "$own_task")" 'run::main(&root, paths, &me)'
called=$(grep -rl --include='*.rs' --exclude-dir=target current_exe "$T_ROOT/console" | grep -vxF -e "$own_lib" -e "$own_task" | while read -r f; do
  rel=${f#"$T_ROOT"/console/}
  name=$(awk -F'\t' -v d="${rel%%/*}" '$1 == d { print $2 }' <<<"$dirs")
  echo "${name:-console}"
done | LC_ALL=C sort -u)
listed=$(grep '^pub const RUNS' "$T_ROOT/console/xtask/src/select.rs" | grep -o '"[a-z-]*"' | tr -d '"' | LC_ALL=C sort -u)
t_eq "$called" "$listed"

t_case "the real workspace: a change to the selector asks for the full gate"
t_eq "$(bash "$T_ROOT/scripts/select-tests.sh" console/xtask/src/select.rs)" "full console/xtask/src/select.rs is part of the gate itself"

t_case "the real checkout: the plain dashboard's module selects the node tests that run it"
picked=$(bash "$T_ROOT/scripts/select-tests.sh" host/fun/dashboard-view.js 2>&1 | grep '^node ')
t_contains "$picked" "node test/dashboard_view_test.mjs"
t_contains "$picked" "node test/dashboard_host_test.mjs"

t_case "the real checkout: a changelog fragment selects the tests that name changelog.d"
picked=$(bash "$T_ROOT/scripts/select-tests.sh" changelog.d/1606.md changelog.d/my-branch.md 2>&1)
t_contains "$picked" "test test/changelog_test.sh"
t_lacks "$picked" "full "

t_case "the real script: a path no test names asks for the full gate, and no path is a usage error"
# Built here, so this file does not name it.
nowhere=nowhere/un"named"
t_eq "$(bash "$T_ROOT/scripts/select-tests.sh" "$nowhere")" "full no test names $nowhere"
t_run "bash '$T_ROOT/scripts/select-tests.sh'"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: scripts/select-tests.sh <path>..."

# verify.sh --changed over a scratch git repository: origin/main is the first
# commit, and what is on top of it is the change.
gitrepo() {
  fresh
  fake a_test.sh "lib/config.sh"
  fake b_test.sh "nothing"
  printf '#!/usr/bin/env bash\n# lib\n' >"$repo/lib/config.sh"
  t_run "cd '$repo' && git init -q && git add -A && git -c user.name=t -c user.email=t@t commit -qm base && git update-ref refs/remotes/origin/main HEAD"
  rm -f "$T"/ran-*
}

t_case "verify.sh --changed runs the test files of an uncommitted change and not the others"
gitrepo
printf '# more\n' >>"$repo/lib/config.sh"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "note  1 changed paths since"
t_contains "$t_out" "1 test files"
t_contains "$t_out" "ok    shellcheck and the shell tests of the change"
t_contains "$t_out" "skip  the workspace's tests (the change touches nothing under console/)"
[ -e "$T/ran-a_test.sh" ] || t_bad "a_test.sh did not run"
[ ! -e "$T/ran-b_test.sh" ] || t_bad "b_test.sh ran"

t_case "verify.sh --changed counts an untracked file as changed"
gitrepo
fake c_test.sh "new"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
[ -e "$T/ran-c_test.sh" ] || t_bad "the new c_test.sh did not run"
[ ! -e "$T/ran-a_test.sh" ] || t_bad "a_test.sh ran"

t_case "verify.sh --changed with no change runs no test file"
gitrepo
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "note  0 changed paths"
t_lacks "$t_out" "shell tests"
[ ! -e "$T/ran-a_test.sh" ] || t_bad "a_test.sh ran"

t_case "verify.sh --changed fails when a selected test file fails"
gitrepo
printf '#!/usr/bin/env bash\necho "FAIL: broke"\nexit 1\n' >"$repo/test/a_test.sh"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  shellcheck and the shell tests of the change"

t_case "verify.sh --changed runs the whole suite when the change needs the full gate"
gitrepo
printf '# changed\n' >>"$repo/test/lib.sh"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_contains "$t_out" "note  the full gate: test/lib.sh is part of the gate itself"
t_contains "$t_out" "ok    shellcheck and the shell tests"
[ -e "$T/ran-a_test.sh" ] && [ -e "$T/ran-b_test.sh" ] || t_bad "not every test file ran"

t_case "verify.sh --changed without origin/main runs the whole suite"
gitrepo
t_run "cd '$repo' && git update-ref -d refs/remotes/origin/main && bash scripts/verify.sh --changed"
t_contains "$t_out" "note  the full gate: there is no merge base with origin/main"
[ -e "$T/ran-b_test.sh" ] || t_bad "b_test.sh did not run"

t_case "verify.sh --changed names the heavy files it leaves to the full gate"
gitrepo
fake update_test.sh "lib/config.sh"
t_run "cd '$repo' && git add -A && git -c user.name=t -c user.email=t@t commit -qm u && git update-ref refs/remotes/origin/main HEAD"
printf '# more\n' >>"$repo/lib/config.sh"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "note  left to the full gate: update_test.sh"
[ ! -e "$T/ran-update_test.sh" ] || t_bad "update_test.sh ran"

t_case "verify.sh --changed tests only the crates of the change, on one line"
gitrepo
workspace
mkdir -p "$repo/console/mcp/src"
echo "// x" >"$repo/console/mcp/src/tools.rs"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "front page files, the crates tehas-mcp"
t_eq "$(grep -c 'the tests of' <<<"$t_out")" 1
t_contains "$t_out" "ok    the tests of tehas-mcp"
t_lacks "$t_out" "the workspace's tests"
t_eq "$(cat "$T/cargo-args")" "test --locked --quiet --manifest-path console/Cargo.toml -p tehas-mcp"

t_case "verify.sh --changed gives cargo every crate of the change"
gitrepo
workspace
mkdir -p "$repo/console/dispatch/src"
echo "// x" >"$repo/console/dispatch/src/poll.rs"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    the tests of store tehas-dispatch"
t_eq "$(cat "$T/cargo-args")" "test --locked --quiet --manifest-path console/Cargo.toml -p store -p tehas-dispatch"

t_case "verify.sh --changed fails when a crate's tests fail"
gitrepo
workspace
touch "$T/cargo-fails"
mkdir -p "$repo/console/mcp/src"
echo "// x" >"$repo/console/mcp/src/tools.rs"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  the tests of tehas-mcp"

t_case "verify.sh --changed tests the whole workspace for a change to the lock"
gitrepo
workspace
mkdir -p "$repo/console"
echo "# x" >"$repo/console/Cargo.lock"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "front page files, the workspace"
t_contains "$t_out" "ok    the workspace's tests"
t_eq "$(cat "$T/cargo-args")" "test --locked --quiet --manifest-path console/Cargo.toml --workspace"

t_case "verify.sh --changed tests the whole workspace when metadata cannot be read"
gitrepo
workspace
echo "not json" >"$T/metadata.json"
mkdir -p "$repo/console/mcp/src"
echo "// x" >"$repo/console/mcp/src/tools.rs"
t_run "cd '$repo' && bash scripts/verify.sh --changed"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    the workspace's tests"
t_eq "$(cat "$T/cargo-args")" "test --locked --quiet --manifest-path console/Cargo.toml --workspace"

t_case "the plain and the full gate test the whole workspace, whatever the change"
for mode in "" --full; do
  gitrepo
  workspace
  mkdir -p "$repo/console/mcp/src"
  echo "// x" >"$repo/console/mcp/src/tools.rs"
  t_run "cd '$repo' && bash scripts/verify.sh $mode"
  t_contains "$t_out" "ok    the workspace's tests"
  t_eq "$(cat "$T/cargo-args")" "test --locked --quiet --manifest-path console/Cargo.toml --workspace"
done

t_done
