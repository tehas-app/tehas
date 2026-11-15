#!/usr/bin/env bash
# Tests for the shell step of scripts/verify.sh --changed: a change whose paths
# hold no test file and nothing to lint is a skip line, never an "ok". Each
# case runs the gate in a scratch repository whose steps are stubs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() { # the exit status of the stub test.sh
  rm -rf "$tree" "$T/test-args"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  for s in check-generic check-bash-lines select-tests; do
    printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/$s.sh"
  done
  printf '%s\n' '#!/usr/bin/env bash' "echo \"\$*\" >'$T/test-args'" "exit $1" >"$tree/scripts/test.sh"
  chmod +x "$tree/scripts/"*.sh
  g init -q -b main
  g add -A
  g commit -q -m base
  g update-ref refs/remotes/origin/main HEAD
}
gate="cd '$tree' && TEHAS_VERIFY_FRESH=1 TEHAS_VERIFY_LOGS='$T/logs' scripts/verify.sh --changed"
t_stub cargo 'exit 0'
t_stub node 'exit 0'

t_case "paths with nothing to test or lint are a skip line, not ok"
fresh 3
echo note >"$tree/notes.md"
t_run "$gate"
t_eq "$t_rc" 0
t_eq "$(cat "$T/test-args")" "notes.md"
t_contains "$t_out" "skip  shellcheck and the shell tests (no test file or shell script among the change)"
t_lacks "$t_out" "ok    shellcheck"

t_case "a change with no path to give is a skip line and runs no test"
fresh 0
t_run "$gate"
t_eq "$t_rc" 0
t_eq "$(test -e "$T/test-args" && echo ran)" ""
t_contains "$t_out" "skip  shellcheck and the shell tests (no test file or shell script among the change)"

t_case "paths that ran a check are ok"
fresh 0
echo note >"$tree/notes.md"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    shellcheck and the shell tests of the change"
t_lacks "$t_out" "skip  shellcheck"

t_case "a failed check fails the gate"
fresh 1
echo note >"$tree/notes.md"
t_run "$gate"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  shellcheck and the shell tests of the change"

t_case "nothing to check in a run that is no --changed one still fails"
fresh 3
t_run "cd '$tree' && TEHAS_VERIFY_FRESH=1 TEHAS_VERIFY_LOGS='$T/logs' scripts/verify.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  shellcheck and the shell tests"

t_done
