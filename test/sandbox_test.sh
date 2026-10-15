#!/usr/bin/env bash
# Tests for host/sandbox/build.sh, against stub git and docker: it builds on
# upstream's image for the head of the branch versions.env names, labels the
# image with that commit, and with --if-moved or --check looks at the label of
# the current image first.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

home=$T/home/tehas
mkdir -p "$home/sandbox"
cp "$T_ROOT/host/sandbox/build.sh" "$home/sandbox/"
cp "$T_ROOT/host/versions.env" "$home/"
sed -i 's#^PI_RUKAS_REPO=.*#PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git#' "$home/versions.env"
build=$home/sandbox/build.sh
fx=$T/fx
mkdir -p "$fx"

head=$(printf 'a%.0s' {1..40})
old=$(printf 'b%.0s' {1..40})
uid=$(id -u)

# git answers the head in $fx/head, nothing when the file is missing. docker
# answers the label in $fx/label, and a pull fails when $fx/pull-fails exists.
t_stub git '[ -e "$T/fx/head" ] || exit 2; printf "%s\trefs/heads/main\n" "$(cat "$T/fx/head")"'
t_stub docker 'case "$1" in
  image) [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
  pull) [ ! -e "$T/fx/pull-fails" ] || exit 1 ;;
esac'
reset() {
  rm -f "$T"/calls/* "$fx"/*
  echo "$head" >"$fx/head"
}

t_case "a build takes upstream's image for the head and labels it with the commit"
reset
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "ls-remote https://github.com/Up-Owner/pi-rukas.git refs/heads/main"
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa --build-arg PI_RUKAS_COMMIT=$head"
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:"
t_contains "$t_out" "with pi-rukas $head, also tagged tehas/pi-rukas:uid$uid"
# A plain build does not ask what the current image is on.
t_lacks "$(t_calls docker)" "image inspect"

t_case "with --if-moved an image already on the head is left alone"
reset
echo "$head" >"$fx/label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas/pi-rukas:uid$uid is on $head, the head of main"
t_lacks "$(t_calls docker)" "build"

t_case "with --if-moved an image on another commit is rebuilt"
reset
echo "$old" >"$fx/label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "PI_RUKAS_COMMIT=$head"

t_case "--check says the head moved and builds nothing"
reset
echo "$old" >"$fx/label"
t_run "$build --check"
t_eq "$t_rc" 1
t_contains "$t_out" "is on $old, main is at $head"
t_eq "$(t_calls docker | grep -c -e '^build' -e '^pull')" 0

t_case "--check of an image with no label says so"
reset
t_run "$build --check"
t_eq "$t_rc" 1
t_contains "$t_out" "is on no known commit"

t_case "without a head nothing is pulled or built"
reset
rm -f "$fx/head"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no head of main in https://github.com/Up-Owner/pi-rukas.git"
t_eq "$(t_calls docker)" ""

t_case "a head whose image is not published yet builds nothing"
reset
: >"$fx/pull-fails"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no image ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa yet"
t_lacks "$(t_calls docker)" "build"

t_case "an argument it does not know is refused"
reset
t_run "$build --pin"
t_eq "$t_rc" 64
t_eq "$(t_calls git)$(t_calls docker)" ""

t_case "the Dockerfile takes its base from the build, not from a pin"
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'FROM ${PI_RUKAS_IMAGE}'
t_lacks "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_COMMIT"
t_contains "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_BRANCH="

t_done
