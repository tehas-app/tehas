#!/usr/bin/env bash
# Tests for lib/rukas.sh: which pi-rukas commit a host follows, against a stub
# git whose ls-remote prints $T/refs. The console reads the same for tehasbot
# clean, so every case of target asks it too and the two answers must be one.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
t_stub git '[ -e "$T/refs" ] || exit 2; cat "$T/refs"'
a=$(printf 'a%.0s' {1..40})
b=$(printf 'b%.0s' {1..40})
c=$(printf 'c%.0s' {1..40})
d=$(printf 'd%.0s' {1..40})
target() { # <PI_RUKAS_FOLLOW>
  T_ENV=(PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git "PI_RUKAS_FOLLOW=$1")
  rm -f "$T"/calls/*
  t_run "\"$T_CONSOLE\" clean target \"\$PI_RUKAS_FOLLOW\" \"\$PI_RUKAS_REPO\""
  local console_rc=$t_rc console_out=$t_out console_calls
  console_calls=$(t_calls git)
  rm -f "$T"/calls/*
  t_run '. "$T_ROOT/lib/rukas.sh"; tehas_rukas_target'
  t_eq "$console_rc" "$t_rc"
  t_eq "$console_out" "$t_out"
  t_eq "$console_calls" "$(t_calls git)"
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

t_case "release:<major.minor> takes the newest tag of that line, not a newer line"
printf '%s\trefs/tags/v0.14.0\n%s\trefs/tags/v0.14.1\n%s\trefs/tags/v0.15.0\n%s\trefs/tags/v10.14.9\n' "$a" "$b" "$c" "$d" >"$T/refs"
target release:0.14
t_eq "$t_rc" 0
t_eq "$t_out" "$b release v0.14.1"

t_case "release:0 takes every minor version before 1.0, and not 1.0"
printf '%s\trefs/tags/v0.14.1\n%s\trefs/tags/v0.16.0\n%s\trefs/tags/v1.0.0\n' "$a" "$b" "$c" >"$T/refs"
target release:0
t_eq "$t_out" "$b release v0.16.0"

t_case "release:<major> takes the newest tag of that major version"
printf '%s\trefs/tags/v2.3.0\n%s\trefs/tags/v2.45.12\n%s\trefs/tags/v3.0.0\n%s\trefs/tags/v12.0.0\n' "$a" "$b" "$c" "$d" >"$T/refs"
target release:2
t_eq "$t_out" "$b release v2.45.12"

t_case "release:<line> with no tag on that line gives nothing"
printf '%s\trefs/tags/v0.15.0\n' "$a" >"$T/refs"
target release:0.14
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
for v in main branch: "" latest release: release:x release:0.14.1 release:v0; do
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

t_case "the image of a commit is upstream's, by the first 12 digits, lower case"
image() { # <PI_RUKAS_REPO> <commit>
  T_ENV=("PI_RUKAS_REPO=$1")
  t_run ". \"\$T_ROOT/lib/rukas.sh\"; tehas_rukas_image '$2'"
}
image https://github.com/Up-Owner/pi-rukas.git "$a"
t_eq "$t_rc" 0
t_eq "$t_out" "ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa"
image https://github.com/up-owner/pi-rukas "$b"
t_eq "$t_out" "ghcr.io/up-owner/pi-rukas:sha-bbbbbbbbbbbb"

t_case "no image is named for a repository off GitHub or a commit that is not 40 hex digits"
for args in "https://example.com/up-owner/pi-rukas.git $a" "git@github.com:up-owner/pi-rukas.git $a" \
  "https://github.com/up-owner/pi-rukas.git abc" "https://github.com/up-owner/pi-rukas.git ${a^^}"; do
  # shellcheck disable=SC2086  # two words
  image $args
  t_eq "$t_rc" 1
  t_eq "$t_out" ""
done

t_done
