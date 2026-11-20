#!/usr/bin/env bash
# Tests for the console in front of tehasbot cycle: what it refuses before
# tehas-cycle runs, and which tehas-cycle and which commands it runs. What
# tehas-cycle does is console/cycle/tests/cycle_*.rs.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
cycle=$home/libexec/cycle
# An open issue, and nothing else of GitHub.
t_stub gh 'case "$1 $2" in "api repos/acme/widget/issues/7") echo "{\"number\":7,\"state\":\"open\"}" ;; esac'
t_stub fake-work
t_stub fake-status
t_stub systemctl
# The sandbox image has no label: no commit of it is on record.
t_stub docker 'exit 1'

# A host with the store, tehas-cycle beside the console, and lib beside
# libexec.
reset() {
  rm -rf "$T"/calls/* "$home"
  mkdir -p "$home/libexec"
  t_store_home "$home"
  # shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
  t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
  cp "$T_CYCLE" "$home/libexec/tehas-cycle"
  t_verb "$cycle" tehasbot cycle
  t_lib_link "$home"
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=true TEHAS_CONSOLE_CMD=true TEHAS_CYCLE_RETRY_WAIT=0)
  t_instance "${T_MINIMAL[@]}"
}

t_case "an invalid instance runs nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

t_case "TEHAS_CYCLE_REPORT set to a value, rust too, stops with 78 before anything runs"
for v in rust bash; do
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_REPORT=$v
  t_run "$cycle 7"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CYCLE_REPORT is no longer read: the cycle's report has one form, so take the line out"
  t_eq "$(t_calls gh)$(t_calls fake-work)" ""
done

t_case "without the program nothing starts: 78, and nothing is asked of GitHub"
reset
rm -f "$home/libexec/tehas-cycle"
t_run "$cycle 7"
t_eq "$t_rc" 78
t_contains "$t_out" "no tehas-cycle at $home/libexec/tehas-cycle"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

t_case "a cycle that runs from its own copy of the code runs that copy's tehas-cycle"
reset
copy=$home/run/cycle-7-1
mkdir -p "$copy/libexec"
t_lib_link "$copy"
ln -s "$cycle" "$copy/libexec/cycle"
printf '%s\n' '#!/usr/bin/env bash' 'echo "$1" >>"$T/calls/copy"' "exec \"$T_CYCLE\" \"\$@\"" >"$copy/libexec/tehas-cycle"
chmod +x "$copy/libexec/tehas-cycle"
rm -f "$home/libexec/tehas-cycle"
t_run "$copy/libexec/cycle 7"
rm -rf "$home/run"
t_eq "$t_rc" 0
t_eq "$(tr '\n' ' ' <"$T/calls/copy")" "cycle "
t_eq "$(t_calls systemctl)" "--user start --no-block tehas-dispatch.service"

t_case "without the test seams it runs the scripts beside it, not names on PATH"
reset
T_ENV=()
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/lib" "$tree/bin"
cp "$cycle" "$tree/libexec/cycle"
cp "$T_ROOT"/lib/*.sh "$tree/lib/"
cp "$T/bin/fake-work" "$tree/bin/tehas"
cp "$T/bin/fake-status" "$tree/bin/tehasbot"
cp "$T_CYCLE" "$tree/libexec/tehas-cycle"
for name in tehas tehasbot tehas-work tehas-status; do t_stub "$name"; done
t_run "$tree/libexec/cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "work 7"
t_eq "$(t_calls fake-status)" "status"
for name in tehas tehasbot tehas-work tehas-status; do t_eq "$(t_calls "$name")" ""; done

t_done
