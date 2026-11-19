#!/usr/bin/env bash
# Tests for test/lib.sh: the deny stubs (a run of t_run that calls docker, gh,
# systemctl, curl or ssh with no t_stub for it reaches a stand-in, not the
# host's program, and the case it ran in fails), the stdin of a run, and
# t_xtask. Each case runs a small test file of its own that sources the
# harness, and reads what that file printed.
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

t_case "a run never reads the stdin the test file was given"
printf '. %q\nt_case "inner"\nt_run "cat"\necho "read=$t_out"\nt_done\n' "$T_ROOT/test/lib.sh" >"$T/inner_test.sh"
t_out=$(echo "the caller's input" | bash "$T/inner_test.sh" 2>&1)
t_contains "$t_out" "read="$'\n'
t_lacks "$t_out" "the caller's input"

# bash -c reads ~/.bashrc when its stdin is a socket and SHLVL is below 2, and
# env -i in t_run drops SHLVL. A runner started with a socket on stdin, as some
# agent tools start a command, then had every run source the scratch home's
# .bashrc: after a first install that is env.sh, which drops GH_TOKEN (#2495).
t_case "a run started with a socket on stdin does not read the home's .bashrc"
if ! command -v python3 >/dev/null 2>&1; then
  t_skip "no python3 to give the test file a socket on stdin"
else
  printf '%s\n' ". $(printf %q "$T_ROOT/test/lib.sh")" 't_case "inner"' \
    'echo "echo read the bashrc" >"$T/home/.bashrc"' 't_run "echo ran"' 'echo "$t_out"' 't_done' \
    >"$T/inner_test.sh"
  t_out=$(python3 -I -c 'import os, socket, sys
a, b = socket.socketpair()
os.dup2(a.fileno(), 0)
os.execvp("bash", ["bash", sys.argv[1]])' "$T/inner_test.sh" 2>&1)
  t_contains "$t_out" "ran"
  t_lacks "$t_out" "read the bashrc"
fi

t_case "t_xtask takes the xtask the run built from T_XTASK_BIN and builds none"
mkdir -p "$T/nocargo"
printf '#!%s\necho "cargo was called" >&2\nexit 1\n' "$T_BASH" >"$T/nocargo/cargo"
printf '#!%s\necho "the xtask of the run"\n' "$T_BASH" >"$T/given-xtask"
chmod +x "$T/nocargo/cargo" "$T/given-xtask"
printf '. %q\nt_case "inner"\nt_xtask || echo "why=$t_xtask_why"\n"$TEHAS_XTASK"\nt_done\n' \
  "$T_ROOT/test/lib.sh" >"$T/inner_test.sh"
t_out=$(PATH="$T/nocargo:$PATH" T_XTASK_BIN="$T/given-xtask" bash "$T/inner_test.sh" 2>&1)
t_contains "$t_out" "the xtask of the run"
t_lacks "$t_out" "cargo was called"
t_lacks "$t_out" "why="

t_case "t_lib_link twice leaves no lib/lib in the checkout it links to"
mkdir -p "$T/fake/lib" "$T/linked"
(T_ROOT=$T/fake; t_lib_link "$T/linked" && t_lib_link "$T/linked")
t_eq "$?" 0
t_eq "$(readlink "$T/linked/lib")" "$T/fake/lib"
t_eq "$(ls -A "$T/fake/lib")" ""

t_case "t_lib_link refuses a directory inside the checkout"
t_out=$(T_ROOT=$T/fake; t_lib_link "$T/fake/lib" 2>&1)
t_eq "$?" 1
t_contains "$t_out" "is inside the checkout"
t_eq "$(ls -A "$T/fake/lib")" ""

t_case "a run's temporary files are under \$T, not in the host's shared /tmp"
t_run 'f=$(mktemp) && echo "$TMPDIR $f"'
t_eq "$t_rc" 0
t_eq "${t_out%% *}" "$T/tmp"
t_eq "$(dirname "${t_out#* }")" "$T/tmp"
# A file that clears $T still gets the directory, and one that names a TMPDIR
# of its own in T_ENV keeps it.
rm -rf "$T/tmp"
T_ENV=(TMPDIR="$T/own")
t_run 'echo "$TMPDIR"; [ -d "$T/tmp" ] && echo made'
t_eq "$t_out" "$T/own
made"
T_ENV=()

t_done
