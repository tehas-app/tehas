#!/usr/bin/env bash
# Tests for tehas outsource: the contractors of a factory, refused with 78
# while the feature is off or still being built, and read and written in the
# state store when a test build turns it on.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

outsource="$T_ROOT/bin/tehas outsource"
home=$T/home/tehas
t_console || t_bad "no console: $t_console_why"
t_store || t_bad "no store: $t_store_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")
t_stub gh

# A store of the factory, made with the feature on as the update makes it.
store_on() {
  rm -rf "$home"
  mkdir -p "$home/libexec" "$home/state"
  cp "$T_STORE" "$home/libexec/store"
  TEHAS_HOME=$home "$home/libexec/store" init --outsource --version v0.30.0 --commit cb6912a \
    --instance example --repo acme/widget >/dev/null
}

t_case "off: every subcommand says so and exits 78, and touches nothing"
rm -rf "$home"
t_instance "${T_MINIMAL[@]}"
T_ENV=()
for sub in "invite" "list" "status" "revoke box1" "drain box1"; do
  t_run "$outsource $sub"
  t_eq "$t_rc" 78
  t_contains "$t_out" "outsource is off"
done
t_eq "$(find "$home" -name 'tehas.db*' 2>/dev/null | wc -l)" 0

t_case "a building feature switched on stops the command at the settings with 78"
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on
t_run "$outsource list"
t_eq "$t_rc" 78
t_contains "$t_out" "outsource is still being built"

t_case "help is the text, and a word that is no subcommand is 64"
t_instance "${T_MINIMAL[@]}"
t_run "$outsource --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas outsource invite [--name NAME] [--json]"
t_run "$outsource promote box1"
t_eq "$t_rc" 64
t_run "$outsource revoke"
t_eq "$t_rc" 64

t_case "on in a test build: invite prints the join command and the store keeps only the code's hash"
store_on
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on TEHAS_OUTSOURCE_PROXY_ADDR=192.0.2.7:8090 TEHAS_OUTSOURCE_CONTRACTORS=box1
T_ENV=(TEHAS_TEST_FEATURES=outsource)
t_run "$outsource invite --name box1"
t_eq "$t_rc" 0
t_eq "$(wc -l <<<"$t_out")" 1
t_contains "$t_out" "tehas-contractor join 192.0.2.7:8090 box1 "
code=${t_out##* }
t_eq "${#code}" 48
t_eq "$(cat "$home"/state/tehas.db* 2>/dev/null | grep -c -a -F "$code")" 0
t_run "$outsource invite --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.api' <<<"$t_out")" 1
t_contains "$(jq -r '.name' <<<"$t_out")" "contractor-"

t_case "on: invite notes an address only this host reaches and a name off the allowed list"
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on
t_run "$outsource invite --name box2"
t_eq "$t_rc" 0
t_contains "$t_out" "127.0.0.1:8090 is TEHAS_OUTSOURCE_PROXY_ADDR, which no other host reaches"
t_contains "$t_out" "box2 is not in TEHAS_OUTSOURCE_CONTRACTORS"
t_contains "$(tail -n 1 <<<"$t_out")" "tehas-contractor join 127.0.0.1:8090 box2 "

t_case "on: list, status, drain and revoke read and write the contractors"
TEHAS_HOME=$home "$home/libexec/store" contractor-add --name box1 --key "$(printf '%064d' 0)" >/dev/null
TEHAS_HOME=$home "$home/libexec/store" contractor-seen --name box1 >/dev/null
t_run "$outsource list --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.contractors[] | {name, status, issue}]' <<<"$t_out")" '[{"name":"box1","status":"idle","issue":null}]'
t_run "$outsource status"
t_eq "$t_out" "outsource: 1 idle, 0 working, 0 resting; 0 cycle(s) outsourced"
t_run "$outsource drain box1"
t_eq "$t_rc" 0
t_run "$outsource list"
t_contains "$t_out" "box1 draining"
t_run "$outsource revoke box1 --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.said' <<<"$t_out")" revoked
t_run "$outsource status --json"
t_eq "$(jq -c '.contractors | {revoked, idle}' <<<"$t_out")" '{"revoked":1,"idle":0}'
t_run "$outsource revoke nobody"
t_eq "$t_rc" 1

t_done
