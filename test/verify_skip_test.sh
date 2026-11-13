#!/usr/bin/env bash
# Tests for what scripts/verify.sh says about a step that ended 0 without
# checking everything it names: the name lint without its list, and the shell
# step whose runner reported a check it did not run. Each case runs the gate in
# a scratch repository whose steps are stubs.
# shellcheck disable=SC2016  # the snippets below are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() {
  rm -rf "$tree"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  cp "$T_ROOT/scripts/check-generic.sh" "$tree/scripts/"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-bash-lines.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'echo "test test/a_test.sh"' >"$tree/scripts/select-tests.sh"
  # The shell suite answers what $T/suite_out holds, so a case can make the
  # runner report checks it did not run.
  cat >"$tree/scripts/test.sh" <<'EOF'
#!/usr/bin/env bash
cat "$T/suite_out" 2>/dev/null
exit 0
EOF
  # The list is gitignored, as it is in the platform checkout, so the gate's
  # tree cannot see it and only its own hash can.
  printf '%s\n' '/scripts/check-generic.names' >"$tree/.gitignore"
  chmod +x "$tree/scripts/"*.sh
  echo one >"$tree/a.txt"
  g init -q -b main
  g add -A
  g commit -q -m base
  g update-ref refs/remotes/origin/main HEAD
}
gate="cd '$tree' && TEHAS_VERIFY_FRESH=1 TEHAS_VERIFY_LOGS='$T/logs' scripts/verify.sh"
names="cd '$tree' && printf '%s\\n' one-two-three >scripts/check-generic.names"
t_stub cargo 'exit 0'
t_stub node 'exit 0'

t_case "without the list the name lint is a skip line, not ok"
fresh
rm -f "$T/suite_out"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "skip  the names in platform files (no names list)"
t_lacks "$t_out" "ok    the names in platform files"

t_case "with the list the name lint is ok"
fresh
t_run "$names"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    the names in platform files"
t_lacks "$t_out" "skip  the names in platform files"

t_case "a hit in the list still fails the step"
fresh
t_run "$names && printf '%s\n' 'the name one-two-three is here' >'$tree/a.txt'"
t_run "$gate"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  the names in platform files"
t_contains "$t_out" "a.txt:1:"

t_case "a runner that skipped a check makes the shell step a skip line"
fresh
rm -f "$T/suite_out"
printf '%s\n' "test.sh: failed checks (0), not run (1)" "SKIPPED: shellcheck is not installed" >"$T/suite_out"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "skip  shellcheck and the shell tests (shellcheck is not installed)"
t_lacks "$t_out" "ok    shellcheck and the shell tests"

t_case "two skipped checks are named in one line"
fresh
rm -f "$T/suite_out"
printf '%s\n' "SKIPPED: shellcheck is not installed" "SKIPPED: something else" >"$T/suite_out"
t_run "$gate"
t_contains "$t_out" "skip  shellcheck and the shell tests (shellcheck is not installed, something else)"

t_case "a run that ran every check is ok"
fresh
printf '%s\n' "test.sh: 3 cases, 0 failed" >"$T/suite_out"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    shellcheck and the shell tests"
t_lacks "$t_out" "skip  shellcheck"

t_case "a failing shell step is still a failure, not a skip"
fresh
cat >"$tree/scripts/test.sh" <<'EOF'
#!/usr/bin/env bash
echo "test.sh: failed checks (1), not run (1)"
echo "SKIPPED: shellcheck is not installed"
echo "FAILED: test/a_test.sh: a case that did not pass"
exit 1
EOF
chmod +x "$tree/scripts/test.sh"
t_run "$gate"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  shellcheck and the shell tests"

t_done
