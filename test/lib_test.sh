#!/usr/bin/env bash
# Tests for the deny stubs of test/lib.sh: a run of t_run that calls docker, gh,
# systemctl, curl or ssh with no t_stub for it reaches a stand-in, not the
# host's program, and the case it ran in fails. Each case runs a small test file
# of its own that sources the harness, and reads what that file printed.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

inner() { # <case body>: run a one-case test file that sources the harness
  printf '. %q\nt_case "inner"\n%s\nt_done\n' "$T_ROOT/test/lib.sh" "$1" >"$T/inner_test.sh"
  t_out=$(bash "$T/inner_test.sh" 2>&1)
  t_rc=$?
}

t_case "a case that calls an unstubbed docker fails, and the host's docker never runs"
inner 't_run "docker image inspect x; echo rc=\$?"; echo "$t_out"'
t_eq "$t_rc" 1
t_contains "$t_out" "rc=97"
t_contains "$t_out" "FAIL: inner: no stub stands in for docker: docker image inspect x"
t_contains "$t_out" "1 cases, 1 failed"

t_case "each of gh, systemctl, curl and ssh is denied too"
inner 't_run "gh auth status; systemctl --user --failed; curl -s u; ssh h true"'
t_eq "$t_rc" 1
for name in gh systemctl curl ssh; do
  t_contains "$t_out" "no stub stands in for $name: $name "
done

t_case "a stub stands in front of the deny stub"
inner 't_stub docker; t_run "docker ps"; t_eq "$t_rc" 0'
t_eq "$t_rc" 0
t_contains "$t_out" "1 cases, 0 failed"

t_case "a stub taken away leaves the deny stub, not the host's program"
inner 't_stub docker; rm -f "$T/bin/docker"; t_run "docker ps"'
t_eq "$t_rc" 1
t_contains "$t_out" "no stub stands in for docker: docker ps"

t_case "a jailed run sees no deny stub"
inner 'T_PLANT=(bash); t_unstub docker; t_run "command -v docker || echo none"; echo "$t_out"'
t_eq "$t_rc" 0
t_contains "$t_out" "none"

t_case "the failure is the case's in which the call was made"
printf '. %q\nt_case "first"\nt_run "curl u"\nt_case "second"\nt_done\n' "$T_ROOT/test/lib.sh" >"$T/inner_test.sh"
t_out=$(bash "$T/inner_test.sh" 2>&1)
t_contains "$t_out" "FAIL: first: no stub stands in for curl: curl u"
t_lacks "$t_out" "FAIL: second"

t_done
