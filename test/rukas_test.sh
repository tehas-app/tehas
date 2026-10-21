#!/usr/bin/env bash
# Tests for lib/rukas.sh: which pi-rukas commit a host follows, against a stub
# git whose ls-remote prints $T/refs.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_stub git '[ -e "$T/refs" ] || exit 2; cat "$T/refs"'
a=$(printf 'a%.0s' {1..40})
b=$(printf 'b%.0s' {1..40})
c=$(printf 'c%.0s' {1..40})
d=$(printf 'd%.0s' {1..40})
target() { # <PI_RUKAS_FOLLOW>
  T_ENV=(PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git "PI_RUKAS_FOLLOW=$1")
  rm -f "$T"/calls/*
  t_run '. "$T_ROOT/lib/rukas.sh"; tehas_rukas_target'
}

t_case "release takes the newest vX.Y.Z tag by version, not by name"
printf '%s\trefs/tags/v0.12.9\n%s\trefs/tags/v0.12.80\n%s\trefs/tags/v0.12.8\n' "$a" "$b" "$c" >"$T/refs"
target release
t_eq "$t_rc" 0
t_eq "$t_out" "$b release v0.12.80"
t_eq "$(t_calls git)" "ls-remote --tags https://github.com/Up-Owner/pi-rukas.git"

t_case "release passes over a tag with a suffix and a tag that is not a version"
printf '%s\trefs/tags/v0.12.80\n%s\trefs/tags/v0.13.0-rc1\n%s\trefs/tags/nightly\n' "$a" "$b" "$c" >"$T/refs"
target release
t_eq "$t_out" "$a release v0.12.80"

t_case "release of an annotated tag is the commit it points at, not the tag object"
printf '%s\trefs/tags/v0.12.79\n%s\trefs/tags/v0.12.80\n%s\trefs/tags/v0.12.80^{}\n' "$a" "$b" "$c" >"$T/refs"
target release
t_eq "$t_out" "$c release v0.12.80"

t_case "release with no version tag gives nothing"
printf '%s\trefs/tags/nightly\n' "$a" >"$T/refs"
target release
t_eq "$t_rc" 1
t_eq "$t_out" ""

t_case "branch:<name> takes the head of that branch"
printf '%s\trefs/heads/main\n' "$d" >"$T/refs"
target branch:main
t_eq "$t_rc" 0
t_eq "$t_out" "$d the head of main"
t_eq "$(t_calls git)" "ls-remote https://github.com/Up-Owner/pi-rukas.git refs/heads/main"

t_case "upstream that does not answer gives nothing"
rm -f "$T/refs"
target release
t_eq "$t_rc" 1
t_eq "$t_out" ""
target branch:main
t_eq "$t_rc" 1
t_eq "$t_out" ""

t_case "a value that is neither release nor branch:<name> is refused without asking upstream"
printf '%s\trefs/heads/main\n' "$d" >"$T/refs"
for v in main branch: "" latest; do
  target "$v"
  t_eq "$t_rc" 1
  t_eq "$t_out" ""
  t_eq "$(t_calls git)" ""
done

t_case "commit:<sha> is that commit and asks upstream nothing"
printf '%s\trefs/tags/v0.12.80\n%s\trefs/heads/main\n' "$a" "$b" >"$T/refs"
for v in "$a" "$b" "$c"; do
  target "commit:$v"
  t_eq "$t_rc" 0
  t_eq "$t_out" "$v pinned"
  # No ls-remote, whatever refs the stub would have answered: the release that
  # wrote the line chose the commit, and asking again could only move it.
  t_eq "$(t_calls git)" ""
done

t_case "a commit that is not 40 lower-case hex digits is refused without asking upstream"
printf '%s\trefs/heads/main\n' "$d" >"$T/refs"
for v in "commit:$a-" "commit:${a:0:39}" "commit:" "commit:$(printf 'A%.0s' {1..40})" "commit:$(printf 'g%.0s' {1..40})"; do
  target "$v"
  t_eq "$t_rc" 1
  t_eq "$t_out" ""
  t_eq "$(t_calls git)" ""
done

t_done
