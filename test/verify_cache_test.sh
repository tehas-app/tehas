#!/usr/bin/env bash
# Tests for the pass that scripts/verify.sh keeps per tree: the same tree in the
# same mode is checked once. Each case runs the gate in a scratch repository
# whose steps are stubs that count their runs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() {
  rm -rf "$tree" "$T/wt" "$T/calls/suite" "$T/calls/cargo" "$T/suite_rc" "$T/touch"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-generic.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-bash-lines.sh"
  # The selection names one test file: a change to scripts/verify.sh itself
  # asks for the whole gate, which would run cargo test whatever the case is
  # about, and some cases count the runs of the workspace's tests.
  printf '%s\n' '#!/usr/bin/env bash' 'echo "test test/a_test.sh"' >"$tree/scripts/select-tests.sh"
  # The names list is ignored, as it is in the platform checkout: the gate's
  # tree cannot see it, so only its own hash tells the two runs apart.
  printf '%s\n' '/scripts/check-generic.names' >"$tree/.gitignore"
  # The shell suite: counts its runs, edits a file mid-run when told to, and
  # exits as $T/suite_rc says.
  cat >"$tree/scripts/test.sh" <<'EOF'
#!/usr/bin/env bash
echo run >>"$T/calls/suite"
[ ! -f "$T/touch" ] || echo more >>"$(cat "$T/touch")"
exit "$(cat "$T/suite_rc" 2>/dev/null || echo 0)"
EOF
  chmod +x "$tree/scripts/"*.sh
  echo one >"$tree/a.txt"
  g init -q -b main
  g add -A
  g commit -q -m base
  g update-ref refs/remotes/origin/main HEAD
}
runs() { t_calls suite | wc -l | tr -d ' '; }
# The workspace's tests are the step that tells a run of the gate from the
# replay of a kept pass: the shell suite cannot, because the gate hands
# scripts/test.sh the mode its parent was called with.
runs_cargo() { t_run "$gate"; t_calls cargo | grep -c '^test ' || true; }
gate="cd '$tree' && scripts/verify.sh"
t_stub node 'exit 0'
# The cargo the gate asks for its own version, for rustfmt's and for clippy's,
# and for the workspace's tests. A '-' stands for a tool the toolchain does not
# hold: the gate keeps no pass made on one toolchain and asks again on another,
# so a case that counts runs gives the same answers every time it calls this.
t_stub_cargo_versions() { # <rustfmt version or '-'> <clippy version or '-'>
  # The body goes in a file, as the cargo stub beside a real workspace does
  # (test/lib.sh, t_stub_cargo): its own $1 and $2 stay the stub's arguments.
  # $T is written as \044T: the double quotes of this function would expand it
  # to the directory of the run that wrote the file, and the stub of a later
  # case has a directory of its own.
  local f=$T/cargo-body
  # shellcheck disable=SC2016  # the quoted parts are the stub's own text
  printf 'case "$1 $2" in\n' >"$f"
  if [ "$1" = - ]; then printf '  "fmt --version") exit 1 ;;\n' >>"$f"
  else printf '  "fmt --version") echo %s; exit 0 ;;\n' "$1" >>"$f"; fi
  if [ "$2" = - ]; then printf '  "clippy --version") exit 1 ;;\n' >>"$f"
  else printf '  "clippy --version") echo %s; exit 0 ;;\n' "$2" >>"$f"; fi
  printf '  "--version") echo "cargo 1.85.0"; exit 0 ;;\nesac\nexit 0\n' >>"$f"
  # The record of a call is t_stub's own line, and the body runs after it, so
  # the calls of the stub stand in t_calls cargo beside what each one printed.
  # shellcheck disable=SC2016  # the line is the stub's text, expanded when it runs
  printf '%s\n' 'echo "cargo $*" >>"$T/calls/cargo"' >>"$f"
  t_stub cargo "exec bash '$f' \"\$@\""
}
t_stub_cargo_versions 'rustfmt 1.8.0' 'clippy 0.1.79'

t_case "a second run on the same tree is a kept pass and runs no step"
fresh
t_run "$gate"
t_eq "$t_rc" 0
t_lacks "$t_out" "passed already"
t_run "$gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_contains "$t_out" "note  this tree passed already"
t_contains "$t_out" "ok    shellcheck and the shell tests"

t_case "a changed tracked file runs the gate again"
fresh
t_run "$gate"
echo two >"$tree/a.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a same-size edit in the second of the last index write runs the gate again"
fresh
# git checks a file whose stat is unchanged only when it is not older than
# the index. The times are pinned so the edit lands in the index's second;
# ctime cannot be pinned, so it is not compared.
g config core.trustctime false
index=$tree/.git/index
touch -d '2026-01-01 00:00:00' "$tree/a.txt"
g update-index -q --refresh
t_run "$gate"
echo two >"$tree/a.txt"
touch -d '2026-01-01 00:00:00' "$tree/a.txt" "$index"
# The real index sees the edit; a status that wrote the index would hide it.
t_eq "$(g --no-optional-locks status --porcelain a.txt)" " M a.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a new untracked file runs the gate again"
fresh
t_run "$gate"
echo new >"$tree/b.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a failure is never kept"
fresh
echo 1 >"$T/suite_rc"
t_run "$gate"
t_eq "$t_rc" 1
t_run "$gate"
t_eq "$t_rc" 1
t_eq "$(runs)" 2
t_contains "$t_out" "FAIL  shellcheck and the shell tests"

t_case "another mode runs the gate again"
fresh
t_run "$gate"
t_run "$gate --full"
t_eq "$t_rc" 0
t_eq "$(runs)" 2
t_run "$gate --full"
t_eq "$(runs)" 2

t_case "TEHAS_VERIFY_FRESH=1 runs every step"
fresh
t_run "$gate"
t_run "export TEHAS_VERIFY_FRESH=1; $gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 2
t_lacks "$t_out" "passed already"

t_case "a tree changed during the run is not kept"
fresh
echo "$tree/a.txt" >"$T/touch"
t_run "$gate"
t_eq "$t_rc" 0
rm "$T/touch"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a worktree of the same repository on the same tree uses the kept pass"
fresh
t_run "$gate"
g worktree add -q --detach "$T/wt" HEAD
t_run "cd '$T/wt' && scripts/verify.sh"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_contains "$t_out" "passed already"

t_case "outside a git repository the gate runs every time"
fresh
rm -rf "$tree/.git"
t_run "GIT_CEILING_DIRECTORIES='$T' $gate"
t_run "GIT_CEILING_DIRECTORIES='$T' $gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 2

t_case "the names list arriving runs the gate again"
fresh
# The stub of the lint is not the lint: the real one of this checkout is the
# file the case runs, so the list can be seen to change what it checks.
cp "$T_ROOT/scripts/check-generic.sh" "$tree/scripts/"
chmod +x "$tree/scripts/check-generic.sh"
t_eq "$(runs_cargo)" 1
# The list holds a name the lint refuses, and a file in the tree names it, so
# the run with the list cannot pass and a replayed pass cannot be read as the
# same key asked for again.
printf '%s\n' 'the name one-two is here' >"$tree/a.txt"
printf '%s\n' one-two >"$tree/scripts/check-generic.names"
t_run "$gate"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  the names in platform files"

t_case "under CI a plain pass is not replayed"
fresh
t_run "$gate"
t_eq "$(runs)" 1
t_run "export CI=1; $gate"
t_eq "$(runs)" 2
t_run "export CI=1; $gate"
t_eq "$(runs)" 2

# rustfmt and clippy are hashed with the other tools: a pass made where one of
# them was missing is not replayed where it answers, and a version change
# counts as another toolchain. The workspace's tests are the step that proves a
# run happened, so the counts are the runs of cargo, not of the shell suite.
t_case "a toolchain that gains rustfmt runs the gate again"
fresh
t_stub_cargo_versions '-' 'clippy 0.1.79'
t_eq "$(runs_cargo)" 1
t_stub_cargo_versions 'rustfmt 1.9.0' 'clippy 0.1.79'
t_eq "$(runs_cargo)" 2

t_case "a toolchain that gains clippy runs the gate again"
fresh
t_stub_cargo_versions 'rustfmt 1.8.0' '-'
t_eq "$(runs_cargo)" 1
t_stub_cargo_versions 'rustfmt 1.8.0' 'clippy 0.1.80'
t_eq "$(runs_cargo)" 2

t_case "the same toolchain is a kept pass"
fresh
t_stub_cargo_versions 'rustfmt 1.8.0' 'clippy 0.1.79'
t_eq "$(runs_cargo)" 1
rm -f "$T/calls/cargo"
t_run "$gate"
t_eq "$(t_calls cargo | grep -c '^test ')" 0
t_contains "$t_out" "passed already"

t_case "a version of rustfmt that goes away runs the gate again"
fresh
t_stub_cargo_versions 'rustfmt 1.8.0' 'clippy 0.1.79'
t_eq "$(runs_cargo)" 1
t_stub_cargo_versions '-' 'clippy 0.1.79'
t_eq "$(runs_cargo)" 2

t_done
