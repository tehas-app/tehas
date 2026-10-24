#!/usr/bin/env bash
# Tests for scripts/select-tests.sh, the paths-to-checks map behind
# `scripts/verify.sh --changed`, and for that mode of the gate. Each case runs
# over a scratch checkout with fake test files of its own, so what is selected
# does not depend on the real suite.
# shellcheck disable=SC2016  # fake files and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

repo=$T/checkout
T_ENV=(PATH="$T/bin:/usr/bin:/bin" TEHAS_TEST_JOBS=1 TMPDIR="$T")

fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/test/fixtures/headless-x" "$repo/lib" "$repo/libexec" "$repo/docs" "$repo/host/fun"
  cp "$T_ROOT/scripts/select-tests.sh" "$T_ROOT/scripts/test.sh" "$T_ROOT/scripts/verify.sh" "$repo/scripts/"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-generic.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-bash-lines.sh"
  chmod +x "$repo/scripts/check-generic.sh" "$repo/scripts/check-bash-lines.sh"
  : >"$repo/test/lib.sh"
  t_stub shellcheck 'exit 0'
}

# A fake test file that names <text> and leaves a mark when it runs.
fake() { # name text
  printf '#!/usr/bin/env bash\n# %s\ntouch "$T/ran-%s"\necho "%s: 1 cases, 0 failed"\n' "$2" "$1" "$1" >"$repo/test/$1"
}

pick() { t_run "cd '$repo' && bash scripts/select-tests.sh $*"; }

t_case "a changed test file selects itself and nothing else"
fresh
fake a_test.sh "lib/config.sh"
fake b_test.sh "nothing"
pick test/b_test.sh
t_eq "$t_out" "test test/b_test.sh"

t_case "a file name with a dot counts as a word, and only as a whole word"
fresh
fake a_test.sh '. "$T_ROOT/lib/config.sh"'
fake b_test.sh "lib/myconfig.sh"
pick lib/config.sh
t_eq "$t_out" "test test/a_test.sh"

t_case "a command counts after its directory, after tehas, or as a test file's name"
fresh
fake a_test.sh '"$T_ROOT/libexec/update" --check'
fake b_test.sh 't_run "tehas update"'
fake update_release_test.sh "nothing"
fake c_test.sh "an update of the list"
fake d_test.sh "tehas updater"
pick libexec/update
t_eq "$t_out" "test test/a_test.sh
test test/b_test.sh
test test/update_release_test.sh"

t_case "a test that loops over every command is selected for any command"
fresh
fake a_test.sh 'for f in "$T_ROOT"/libexec/*; do bash -n "$f"; done; libexec/"$name"'
pick libexec/work
t_eq "$t_out" "test test/a_test.sh"

t_case "a helper that names the path brings in the test files that use the helper"
fresh
printf '# runs lib/disk.sh\n' >"$repo/test/disk_lib.sh"
fake a_test.sh ". disk_lib.sh"
fake b_test.sh "nothing"
pick lib/disk.sh
t_eq "$t_out" "test test/a_test.sh"

t_case "a changed helper selects the test files that name it"
fresh
printf '# a helper\n' >"$repo/test/disk_lib.sh"
fake a_test.sh ". disk_lib.sh"
fake b_test.sh "nothing"
pick test/disk_lib.sh
t_eq "$t_out" "test test/a_test.sh"

t_case "a path under console/ selects the workspace and the shell tests that build it"
fresh
fake a_test.sh "t_store || t_skip"
fake b_test.sh "console/target"
fake c_test.sh "nothing"
pick console/store/src/main.rs
t_eq "$t_out" "cargo
test test/a_test.sh
test test/b_test.sh"

t_case "a fixture selects the test files that name its directory"
fresh
fake a_test.sh '"$T_ROOT/test/fixtures/headless-x/progress.jsonl"'
fake b_test.sh "nothing"
pick test/fixtures/headless-x/progress.jsonl
t_eq "$t_out" "test test/a_test.sh"

t_case "a front page file selects the node test that loads it"
fresh
printf '// import("../host/fun/brick-wall.js")\n' >"$repo/test/brick_wall_test.mjs"
fake a_test.sh "nothing"
pick host/fun/brick-wall.js
t_eq "$t_out" "node test/brick_wall_test.mjs"

t_case "the heavy files a shared library reaches are left to the full gate"
fresh
for f in install_steps_test.sh update_test.sh update_release_test.sh pack_release_test.sh bootstrap_test.sh a_test.sh; do
  fake "$f" '. "$T_ROOT/lib/config.sh"'
done
pick lib/config.sh
t_eq "$t_out" "test test/a_test.sh
omit test/bootstrap_test.sh
omit test/install_steps_test.sh
omit test/pack_release_test.sh
omit test/update_release_test.sh
omit test/update_test.sh"

t_case "a heavy file runs when the change touches what it tests"
fresh
fake install_steps_test.sh '"$T_ROOT/install.sh"; "$T_ROOT/libexec/update"'
fake update_test.sh '"$T_ROOT/libexec/update"'
fake pack_release_test.sh '"$T_ROOT/scripts/pack-release.sh"'
fake bootstrap_test.sh '"$T_ROOT/bootstrap.sh"'
pick libexec/update scripts/pack-release.sh bootstrap.sh
t_eq "$t_out" "test test/bootstrap_test.sh
omit test/install_steps_test.sh
test test/pack_release_test.sh
test test/update_test.sh"
pick install.sh
t_eq "$t_out" "test test/install_steps_test.sh"

t_case "a heavy file runs when it or a helper it uses changed"
fresh
printf '# a helper\n' >"$repo/test/install_lib.sh"
fake install_steps_test.sh ". install_lib.sh"
fake update_test.sh "nothing"
pick test/install_lib.sh test/update_test.sh
t_eq "$t_out" "test test/install_steps_test.sh
test test/update_test.sh"

t_case "a change to test/lib.sh still runs the heavy files, in the full gate"
fresh
fake install_steps_test.sh "x"
pick test/lib.sh
t_eq "$t_out" "full test/lib.sh is part of the gate itself"

t_case "the gate's own files ask for the full gate"
for p in scripts/test.sh scripts/verify.sh scripts/select-tests.sh test/lib.sh; do
  fresh
  fake a_test.sh "nothing"
  pick "$p test/a_test.sh"
  t_eq "$t_out" "full $p is part of the gate itself"
done

t_case "a path no test names asks for the full gate"
fresh
fake a_test.sh "nothing"
pick host/sandbox/Dockerfile
t_eq "$t_out" "full no test names host/sandbox/Dockerfile"

t_case "records under docs/ and the top-level notes need no check when no test names them"
fresh
fake a_test.sh "nothing"
pick docs/plans/x.md README.md
t_eq "$t_out" ""
t_eq "$t_rc" 0

t_case "a deleted test file selects nothing"
fresh
fake a_test.sh "nothing"
pick test/gone_test.sh
t_eq "$t_out" ""

t_case "no path is a usage error"
fresh
pick ""
t_eq "$t_rc" 64

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

t_done
