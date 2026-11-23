#!/usr/bin/env bash
# Tests for install.sh, split from install_settings_test.sh: the console this
# tree builds judges the settings, and --check takes the one it finds.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# The tests name a console in TEHAS_SETTINGS_CMD; these cases take it away.
no_hook='unset TEHAS_SETTINGS_CMD; '
real_console

t_case "a real run builds this tree's console before it reads the settings"
fresh
instance
t_run "$no_hook$install"
t_eq "$t_rc" 0
t_contains "$(t_calls cargo | head -n 1)" "-p console"
t_eq "$([ -x "$th/libexec/console" ] && echo installed)" installed

t_case "the built console judges the settings, and a refusal installs nothing"
fresh
instance TEHAS_WORK_MEMORY=8g
t_run "$no_hook$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORK_MEMORY is set"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a console that does not build stops the run before the settings, with nothing installed"
fresh
instance
t_env CARGO_FAIL=console
t_run "$no_hook$install"
t_env
t_eq "$t_rc" 1
t_contains "$t_out" "the console did not build"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a build that fails leaves the program that was there, for the run that holds the lock"
fresh
instance
mkdir -p "$T/target/release"
echo holder >"$T/target/release/console"
t_env CARGO_FAIL=console
t_run "$no_hook$install"
t_env
t_eq "$t_rc" 1
t_eq "$(cat "$T/target/release/console")" holder
rm -rf "$T/target"

t_case "a home that is not tehas's is refused before the console is built"
fresh
instance
mkdir -p "$th"
echo mine >"$th/notes.txt"
t_run "$no_hook$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_HOME is not empty and holds no .tehas-home"
t_contains "$t_out" "nothing was installed"
t_eq "$(t_calls cargo)" ""
rm -f "$th/notes.txt"

t_case "a TEHAS_BUILD_TIMEOUT that is no number stops the first build: 78, and no cargo"
fresh
instance
t_env TEHAS_BUILD_TIMEOUT=abc
t_run "$no_hook$install"
t_env
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_BUILD_TIMEOUT is not a number of seconds: abc"
t_eq "$(t_calls cargo)" ""

t_case "the home's old console does not judge a tree it did not come from"
fresh
instance
mkdir -p "$th/libexec"
printf '#!%s\necho old-console-ran >&2\nexit 2\n' "$T_BASH" >"$th/libexec/console"
chmod +x "$th/libexec/console"
t_run "$no_hook$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "old-console-ran"

t_case "a later command of the run does not keep the console of the build"
t_contains "$(sed -n '/^tehas_load$/,/^instance=/p' "$install")" 'unset TEHAS_SETTINGS_CMD'

t_case "--check builds nothing: with no console anywhere the settings are not judged, and it says so"
fresh
instance
rm -rf "$T/target"
t_run "$no_hook$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "no console at $th/libexec/console to load the settings: run install.sh"
t_eq "$(t_calls cargo)" ""

t_case "--check takes the console an earlier build left"
fresh
instance
mkdir -p "$T/target/release"
cp "$T_CONSOLE" "$T/target/release/console"
t_run "$no_hook$install --check"
t_lacks "$t_out" "INVALID"
t_eq "$(t_calls cargo)" ""

t_case "--check with no build takes the installed console"
fresh
instance
rm -rf "$T/target"
mkdir -p "$th/libexec"
cp "$T_CONSOLE" "$th/libexec/console"
t_run "$no_hook$install --check"
t_lacks "$t_out" "INVALID"
t_eq "$(t_calls cargo)" ""

t_done
