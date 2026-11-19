#!/usr/bin/env bash
# Tests for install.sh --lab-build: the programs are built with the lab marker
# (TEHAS_LAB_BUILD=on) and without debug assertions, so they read
# TEHAS_TEST_FEATURES and none of the tests' other hooks; a plain run builds
# them again; neither takes the marker or debug assertions from the caller's
# environment; a release tree is refused; and a lab environment file a plain
# run finds is named for removal.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

lab_env=$home/.config/environment.d/tehas-lab.conf

t_case "a lab build builds every program with the lab marker, without debug assertions, and says so"
fresh
instance
t_run "$install --lab-build"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/console")" "a console built from source as a lab build"
t_eq "$(cat "$th/libexec/tehas-dispatch")" "a tehas-dispatch built from source as a lab build"
t_eq "$(cat "$th/libexec/tehas-mcp")" "a tehas-mcp built from source as a lab build"
t_contains "$t_out" "note: a lab build: its programs read TEHAS_TEST_FEATURES, from $lab_env for the units"
t_lacks "$t_out" "later: rm $lab_env"

t_case "a second lab build changes nothing"
t_run "$install --lab-build"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas is up to date"

t_case "a plain run after a lab build builds every program again, and --check finds the lab build first"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $th/libexec/console"
: >"$T/calls/cargo"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(cat "$th/libexec/tehas-cycle")" "a tehas-cycle built from source"
t_contains "$(t_calls cargo)" "-p tehas-cycle"
t_lacks "$t_out" "note: a lab build"

t_case "no run takes the lab marker or debug assertions from the environment"
T_ENV=(CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true TEHAS_LAB_BUILD=on)
t_run "$install --lab-build"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/console")" "a console built from source as a lab build"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/console")" "a console built from source"
T_ENV=()

t_case "a plain run names a lab environment file it finds, and a lab build does not"
mkdir -p "${lab_env%/*}"
echo "TEHAS_TEST_FEATURES=outsource" >"$lab_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: rm $lab_env && systemctl --user daemon-reload   (this is no lab build, so TEHAS_TEST_FEATURES does nothing)"
t_run "$install --lab-build"
t_eq "$t_rc" 0
t_lacks "$t_out" "later: rm $lab_env"
# The file is the person's: no run removes it.
t_eq "$(cat "$lab_env")" "TEHAS_TEST_FEATURES=outsource"

t_case "a release tree is refused with 78 before anything is written or built"
fresh
instance
released=$T/released
rm -rf "$released"
cp -R "$fixture" "$released"
rm -rf "$released/console"
: >"$T/calls/cargo"
t_run "$released/install.sh --lab-build"
t_eq "$t_rc" 78
t_contains "$t_out" "--lab-build builds from the source in console/, and $released has none (a release tree); nothing was installed"
t_eq "$(t_calls cargo)" ""
t_eq "$(find "$home" -mindepth 1 | wc -l)" 0

t_done
